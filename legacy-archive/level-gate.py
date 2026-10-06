#!/usr/bin/env python3
"""Rebuild the working tree at feature level N (1..4) starting from HEAD.

Levels: 1 = A11 URL policy, 2 = generated-link integrity, 3 = citations and
glossary, 4 = the A12 attribute-injection fix.

Every level is an explicit list of anchored edits against HEAD rather than a
partition of git's hunks: the hunks for convert.rs, options.rs and cli.rs
straddle feature boundaries, so no hunk-level cut separates them. Each anchor
must occur exactly once or the build stops, so a stale anchor fails loudly
instead of yielding a tree that compiles for the wrong reason.

Invariant: level 4 reproduces /tmp/final byte for byte.
"""
import os
import re
import subprocess
import sys

REPO = "/home/vicpu/build"
FINAL = "/tmp/final"
LEVEL = int(sys.argv[1])

TRACKED = [
    "textrill/src/lib.rs",
    "textrill/src/links.rs",
    "textrill/src/options.rs",
    "textrill/src/cli.rs",
    "textrill/src/convert.rs",
    "textrill/src/main.rs",
    "textrill/src/template.rs",
    "textrill/tests/linktest.rs",
    "textrill/tests/corpus/cases.sh",
    "textrill/tests/optionstest.rs",
    "textrill-gui-rs/tests/acceptance.rs",
    "textrill/README.md",
    "REMEDIATION-PLAN.md",
    "ADVERSARIAL-FINDINGS.md",
]

# Files that enter the tree whole, at the level given.
WHOLE = {
    "textrill/src/urlscheme.rs": 1,
    "textrill/tests/urlschemetest.rs": 1,
    "textrill/tests/corpus/inputs/url_scheme.txt": 1,
    "textrill/tests/linkintegrity.rs": 2,
    "textrill/src/notes.rs": 3,
    "textrill/tests/notest.rs": 3,
}

COUNTS = {1: (63, 118), 2: (63, 118), 3: (65, 121), 4: (65, 121)}
A12_MARKER = "// ------------------------------------------- attribute injection into a tag"


BASE = "2ae6b89f2a179fe00af11886c2aadd9fbdb017d8"


def git(*a):
    return subprocess.run(("git",) + a, cwd=REPO, capture_output=True, text=True, check=True).stdout


def once(t, anchor, what):
    n = t.count(anchor)
    if n != 1:
        raise SystemExit(f"{what}: anchor occurs {n}x, want 1\n  {anchor!r}")
    return t


def after(t, anchor, add, what):
    return once(t, anchor, what).replace(anchor, anchor + add, 1)


def before(t, anchor, add, what):
    return once(t, anchor, what).replace(anchor, add + anchor, 1)


def sub(t, old, new, what):
    return once(t, old, what).replace(old, new, 1)


def subn(t, old, new, n, what):
    if t.count(old) != n:
        raise SystemExit(f"{what}: anchor occurs {t.count(old)}x, want {n}\n  {old!r}")
    return t.replace(old, new)


# ------------------------------------------------------------ shared text blocks
# options.rs: the `validate()` tail and the two new functions that follow it. This
# region is written as one substitution rather than separate inserts, because the
# recorded hunk adds a block whose closing `Ok(())` / `}` come from HEAD -- splitting
# it into inserts produces unbalanced braces.

VALIDATE_TAIL_HEAD = """        }
        self.validate_template_options()?;
        Ok(())
    }
"""

VALIDATE_TAIL_1 = """        }
        self.validate_style_url()?;
        self.validate_template_options()?;
        Ok(())
    }

    /// The scheme policy a conversion runs under.
    ///
    /// Read once per conversion and handed to both the dictionary loader and the
    /// scrub pass, so the two cannot disagree about what is allowed.
    pub fn url_policy(&self) -> crate::urlscheme::UrlPolicy {
        match &self.allowed_url_schemes {
            // An empty or blank list is the default tier too, so a value that
            // has been persisted and read back unchanged cannot turn a
            // front end's "unset" into something weaker.
            Some(list) => crate::urlscheme::UrlPolicy::strict(list),
            None => crate::urlscheme::UrlPolicy::default(),
        }
    }

    /// A11. `--style_url` is the one `href` that is pure operator input, so it
    /// is refused outright rather than scrubbed: there is no document text to
    /// preserve and a silent drop would leave the operator wondering why their
    /// stylesheet is not applied.
    ///
    /// A relative `--style_url` (what most callers want, and what the templates
    /// in `tests/` use) has no scheme and always passes.
    fn validate_style_url(&self) -> Result<(), String> {
        if self.style_url.is_empty() {
            return Ok(());
        }
        let policy = self.url_policy();
        if policy.allows(&self.style_url) {
            return Ok(());
        }
        Err(format!(
            "--style_url {:?} uses a scheme this conversion refuses ({}). \\
             Name it in --allowed_url_schemes to keep the stylesheet.",
            self.style_url,
            policy.describe()
        ))
    }
"""

NOTES_VALIDATE_FN = """
    /// Refuse the note modes with the two that stream.
    ///
    /// Both write output as they go, and a note list is only known once the
    /// whole document has been read: numbering follows first reference, so a
    /// citation in the last paragraph can insert `[1]` in the first. Refusing
    /// here, before any byte is written, is the only way to keep the promise
    /// that a broken note set produces no output at all.
    fn validate_notes_options(&self) -> Result<(), String> {
        for (flag, on) in [
            ("--citations", self.citations),
            ("--glossary", self.glossary),
        ] {
            if !on {
                continue;
            }
            if self.chunk {
                return Err(format!("{flag} is not valid with --chunk"));
            }
            if self.stream {
                return Err(format!("{flag} is not valid with --stream"));
            }
        }
        Ok(())
    }
"""

VALIDATE_TAIL_3 = (
    VALIDATE_TAIL_1.replace(
        "        self.validate_template_options()?;\n        Ok(())",
        "        self.validate_template_options()?;\n"
        "        self.validate_notes_options()?;\n        Ok(())",
    )
    + NOTES_VALIDATE_FN
)

ALLOWED_FIELD = """    /// A11. Schemes permitted in a generated `href`, comma separated.
    ///
    /// The default is [`urlscheme::DEFAULT_ALLOWED_SCHEMES`] spelled out rather
    /// than "unset", and that is deliberate. A front end reads the defaults with
    /// `cli::get_value` and writes them back, and it persists them to its own
    /// settings file. With an "unset means the default" encoding, the empty
    /// string that `get_value` returns for unset would be written back as an
    /// *explicitly empty* list, and an empty list means "refuse nothing" -- so
    /// saving the settings once would quietly turn the policy off. Carrying the
    /// list makes the round trip lossless in meaning as well as in text.
    ///
    /// An anchor using any other scheme is unwrapped -- the tags go, the text
    /// stays -- and the scheme is reported once on standard error.
    ///
    /// Setting this **replaces** the list rather than adding to it, so
    /// `https` alone also stops `http`, `ftp` and `mailto` from being linked.
    /// That is the useful direction: the option's job is to be able to make the
    /// policy stricter, and an additive option could not. The diagnostics say so
    /// on every rule they drop, because forgetting the other schemes is the
    /// obvious mistake.
    ///
    /// This exists because two of the engine's inputs are not the operator's
    /// text: the document being converted, and a link dictionary. `<URL:…>`
    /// in ordinary prose became a live `javascript:` link in the reference, so
    /// the default refuses every script-bearing scheme. See
    /// [`crate::urlscheme`] for why the check is a scan over the finished markup.
    pub allowed_url_schemes: Option<Vec<String>>,
"""

ALLOWED_DEFAULT = """            // A11. `None` is the default tier, not "allow everything" -- see
            // the field's doc comment. An empty list resolves to the same tier,
            // which is what makes a front end that persists this value safe.
            allowed_url_schemes: None,
"""

CITATIONS_FIELD = """    /// Collect `{{textrill:cite:key}}` references into a numbered endnotes list.
    ///
    /// The source syntax, the rendering and the reasons each mistake is refused
    /// rather than guessed at are in [`crate::notes`]. Default **off**: with the
    /// mode off every marker is left in the output as literal text, so a document
    /// that does not use notes converts byte for byte as before.
    ///
    /// Refused with `--chunk` and `--stream`. `--chunk` would have to collect
    /// across the whole input to number references before splitting it, and
    /// `--stream` has already written bytes by the time the list is complete.
    /// Both would need a document-wide pass, which is the one thing those modes
    /// exist to avoid.
    pub citations: bool,
"""

GLOSSARY_FIELD = """    /// Collect `{{textrill:gloss:term}}` references into a definition list.
    ///
    /// Independent of [`Options::citations`]: either mode, both or neither may
    /// be on. Markers of a mode that is off stay literal text, so enabling
    /// `--citations` never draws the glossary markers into it. Default **off**.
    /// Refused with `--chunk` and `--stream`, for the reasons given on
    /// [`Options::citations`].
    pub glossary: bool,
"""


# The README's test count is measured per level after the tree builds, so it is
# patched in a second pass by `count_tests` rather than hard-coded here.
COUNTS = {1: (63, 118), 2: (63, 118), 3: (65, 121), 4: (65, 121)}
COUNTS_ENGINE = {1: 258, 2: 263, 3: 304, 4: 306}
RUST_TESTS = {1: None, 2: None, 3: None, 4: 306}
CORPUS_CASES = {1: 60, 2: 60, 3: 60, 4: 60}

CITATIONS_SECTION = "### Citations and glossary\n\nTwo opt-in, default-off modes collect explicit references into a list at the end\nof the body. Both are triggered only by a namespaced marker; ordinary prose is\nnever interpreted, so `[^1]`, `^2`, `(3)`, `[4]` and `@five` stay text.\n\n| Marker | Meaning |\n| --- | --- |\n| `{{textrill:cite:key}}` | Cite `key` |\n| `{{textrill:def:cite:key}}` … `{{/textrill:def:cite:key}}` | The text of that citation |\n| `{{textrill:gloss:key}}` | Refer to the term `key` |\n| `{{textrill:def:gloss:key}}` … `{{/textrill:def:gloss:key}}` | The definition of that term |\n\n```sh\ntextrill --citations <<'EOF'\nSee the study {{textrill:cite:knuth}} and again {{textrill:cite:knuth}}.\n\n{{textrill:def:cite:knuth}}\nKnuth, *Literate Programming*.\n{{/textrill:def:cite:knuth}}\nEOF\n```\n\nCitations become `[1]`, `[2]`, … numbered by first reference, with the list\nappended as an endnotes `<ol>`; terms become a `<dl>` whose `<dt>` is the key.\nBoth link to their entry and each entry links back to its first reference. Only\nthe first reference to a key carries an `id`, so fifty mentions of one citation\nstill produce one target.\n\nA definition may use textrill's own delimiters (`*italic*`, `` `code` ``), since\nit is collected from the converted document.\n\nWith both modes off the markers are left alone and the output is byte-identical\nto a run without them. A marker belonging to a mode that is *off* is also left\nalone, so `--citations` never turns a glossary marker into an error.\n\nAnything ambiguous is refused, and the message goes to standard error with a\nnon-zero exit **before the output file is opened** — a document with a broken\nnote set produces no output rather than a `[1]` pointing at nothing:\n\n- a reference with no definition, or a definition nothing references\n- a definition given twice, or given empty\n- an unbalanced block, or a closing tag that does not match the block it closes\n- a key that is empty or uses anything but ASCII letters, digits, `-`, `_`, `.`\n  (keys land in `id` attributes)\n- a `{{textrill:…}}` token that is not one of the four above\n\n`--chunk` and `--stream` are refused with either mode: numbering depends on the\nwhole document, and both write output as they go. `--extract` works, since the\nlists simply append to the body.\n\nWith `--template`, `{{textrill:citations}}` and `{{textrill:glossary}}` place\nthe two lists wherever the template wants them. A template that names neither\nslot still gets them appended, so an existing template never loses a list.\n\nNo JavaScript is involved, and no CSS either: the note body is emitted once, as a\nreal list, rather than once per reference.\n"

EDITS = []


def edit(level, path):
    def deco(fn):
        EDITS.append((level, path, fn))
        return fn

    return deco


# =========================================================== level 1: A11 URL policy


@edit(1, "textrill/src/lib.rs")
def _(t):
    t = sub(t, "//! Five deliberate deviations", "//! Six deliberate deviations", "lib.dev")
    t = after(
        t,
        "//!   exactly as the reference does.\n",
        "//! * A `href` whose URL names a script-bearing scheme \u2014 `javascript`, `data`,\n"
        "//!   `vbscript`, `file` \u2014 is unwrapped rather than emitted; the visible words\n"
        "//!   stay. See [`urlscheme`] for why the check is a scan over finished markup and\n"
        "//!   why the default refuses a small named set rather than allowlisting one.\n"
        "//!\n"
        "//!   The document chooses these URLs. Perl never looked at them, so\n"
        "//!   `<URL:javascript:alert(document.domain)>` in an otherwise ordinary file\n"
        "//!   came out of the reference as a live link, and a link dictionary can name\n"
        "//!   any URL at all. `--allowed_url_schemes` sets a strict allowlist instead,\n"
        "//!   which is the stronger property, and is the right choice for a site that\n"
        "//!   does not use custom schemes. **The two implementations therefore cannot\n"
        "//!   agree** on a document containing one of these, so the `url_scheme` corpus\n"
        "//!   case is declared with `differential must fail:` \u2014 the divergence is the\n"
        "//!   fix, and a byte comparison is not the oracle.\n"
        "//!\n"
        "//!   Two related things are *not* deviations, because the reference cannot be\n"
        "//!   compared to them: the refused scheme's diagnostic, and the diagnostic for\n"
        "//!   a dictionary rule dropped at load. Both are new output on stderr.\n",
        "lib.dev_doc",
    )
    return after(t, "pub mod template;\n", "pub mod urlscheme;\n", "lib.mod")


@edit(1, "textrill/src/links.rs")
def _(t):
    t = after(
        t,
        "    pub rejected_patterns: Vec<String>,\n",
        "    /// A11. Which schemes may reach an `href`. See [`crate::urlscheme`].\n"
        "    pub policy: crate::urlscheme::UrlPolicy,\n",
        "links.field",
    )
    t = sub(
        t,
        "    pub fn new(lower_case_tags: bool) -> Self {",
        "    pub fn new(lower_case_tags: bool, policy: crate::urlscheme::UrlPolicy) -> Self {",
        "links.sig",
    )
    t = after(t, "            rejected_patterns: Vec::new(),\n", "            policy,\n", "links.init")
    t = after(
        t,
        "        self.label_seen.insert(label.to_string());\n",
        "\n"
        "        // A11. A rule whose URL is written out in full is checked here, at load,\n"
        "        // where the operator can still fix it: a diagnostic naming the\n"
        "        // dictionary is worth more than silently unwrapping every match.\n"
        "        //\n"
        "        // Two forms are not checked, and neither is a gap. A URL containing a\n"
        "        // capture reference (`$1`, `$&`) is only known at substitution time, and\n"
        "        // a `-h->` rule's URL is raw HTML rather than a URL at all; both are\n"
        "        // caught by the scrub over the finished paragraph, which\n"
        "        // `Converter::apply_links` runs.\n"
        "        if switches & LINK_HTML == 0 && !url.contains('$') && !self.policy.allows(url) {\n"
        "            let scheme = crate::urlscheme::scheme_of(url).unwrap_or_default();\n"
        "            let msg = format!(\n"
        '                "textrill: ignoring link-dictionary rule {label:?}: its URL uses the \\\n'
        '                 {scheme:?} scheme, which this conversion refuses ({}). \\\n'
        '                 Name it in --allowed_url_schemes to keep the rule.",\n'
        "                self.policy.describe()\n"
        "            );\n"
        '            eprintln!("{msg}");\n'
        "            self.rejected_patterns.push(msg);\n"
        "            return;\n"
        "        }\n",
        "links.load_check",
    )
    n = t.count("LinkParser::new(opts.lower_case_tags)")
    if n != 2:
        raise SystemExit(f"links.new_call: found {n} sites, want 2")
    return t.replace(
        "LinkParser::new(opts.lower_case_tags)",
        "LinkParser::new(opts.lower_case_tags, opts.url_policy())",
    )


@edit(1, "textrill/src/options.rs")
def _(t):
    # The struct fields are alphabetical, and the three new ones are inserted at
    # their sorted position rather than appended.
    t = after(t, "pub struct Options {\n", ALLOWED_FIELD, "opt1.allowed_field")
    t = after(t, "        Options {\n", ALLOWED_DEFAULT, "opt1.allowed_default")
    return sub(t, VALIDATE_TAIL_HEAD, VALIDATE_TAIL_1, "opt1.validate_tail")


@edit(3, "textrill/src/options.rs")
def _(t):
    t = after(t, "    pub bold_delimiter: String,\n", CITATIONS_FIELD, "opt3.citations_field")
    t = after(t, "    pub extract: bool,\n", GLOSSARY_FIELD, "opt3.glossary_field")
    t = after(t, "            toc: false,\n", "            citations: false,\n", "opt3.citations_default")
    t = after(t, "            hrule_min: 4,\n", "            glossary: false,\n", "opt3.glossary_default")
    return sub(t, VALIDATE_TAIL_1, VALIDATE_TAIL_3, "opt3.validate_tail")


@edit(1, "textrill/src/cli.rs")
def _(t):
    t = after(
        t,
        'pub const SPECS: &[Spec] = specs![\n',
        '    Str "Allow only these URL schemes in href values; a stricter policy than the '
        'default, which refuses script-bearing schemes (A11)." ["allowed_url_schemes", "url_schemes"],\n',
        "cli.spec",
    )
    t = before(
        t,
        '        "append_file" => opts.append_file.clone(),\n',
        "        // A11. Comma separated, matching what `set_str` splits on. `Options`\n"
        "        // carries the standard list rather than \"unset\", so this round trips to\n"
        "        // the same policy and not to an empty one.\n"
        "        // `None` and `[]` both round-trip as \"\", and `set_value(\"\")` resolves\n"
        "        // to the default tier, so a front end can persist the unset state\n"
        '        // without having to know what "unset" means.\n'
        '        "allowed_url_schemes" => opts.allowed_url_schemes.as_deref().unwrap_or(&[]).join(","),\n',
        "cli.get_value",
    )
    return before(
        t,
        '        "append_file" => opts.append_file = v.to_string(),\n',
        "        // A11. **Replaces** rather than accumulates, unlike the repeatable\n"
        "        // options (`--infile`, `--links_dictionaries`). A comma-separated list\n"
        "        // is one value, and a front end must be able to write the whole thing\n"
        "        // back; accumulating would double the list on every round trip through\n"
        "        // `get_value`. Layering still overrides in the usual way, since the\n"
        "        // command line is applied after the rc files.\n"
        '        "allowed_url_schemes" => {\n'
        "            opts.allowed_url_schemes = Some(v.split(',').map(|s| s.trim().to_string()).collect());\n"
        "        }\n",
        "cli.set_str",
    )


@edit(1, "textrill/src/convert.rs")
def _(t):
    t = after(
        t,
        "    document_template: bool,\n",
        "    /// A11. The scheme policy, built once in [`Converter::new`] so the scrub\n"
        "    /// does not rebuild it per paragraph. Kept beside `links` because the two\n"
        "    /// must agree; `links::load_links` is given this same value.\n"
        "    url_policy: crate::urlscheme::UrlPolicy,\n"
        "    /// A11. Schemes already reported on standard error, so a document with two\n"
        "    /// hundred `javascript:` links says so once instead of two hundred times.\n"
        "    dropped_schemes: Vec<String>,\n",
        "convert.fields",
    )
    t = before(
        t,
        "        let links = links::load_links(&opts);\n",
        "        // A11. One policy, shared with the dictionary loader, so a rule kept at\n"
        "        // load and a paragraph scrubbed later can never disagree.\n"
        "        let url_policy = opts.url_policy();\n",
        "convert.new_policy",
    )
    t = after(
        t,
        "            template_text,\n",
        "            url_policy,\n            dropped_schemes: Vec::new(),\n",
        "convert.init",
    )
    return after(
        t,
        "            self.links.check_dictionary_links(para_ref);\n",
        "            // A11. The single point where document text and dictionary URLs\n"
        "            // become an `href`, so it is also the single point that decides\n"
        "            // whether the scheme is one a browser will act on. Placed here\n"
        "            // rather than at each construction site so that `links.rs` stays a\n"
        "            // faithful port and so a producer added later is covered without\n"
        "            // remembering this rule.\n"
        "            //\n"
        "            // Runs over the finished paragraph rather than per substitution, so\n"
        "            // it costs one scan per paragraph and not one per match -- the same\n"
        "            // reasoning as the P6 prefilter.\n"
        "            let mut dropped = Vec::new();\n"
        "            if let Some(scrubbed) =\n"
        "                crate::urlscheme::scrub_hrefs(para_ref, &self.url_policy, &mut dropped)\n"
        "            {\n"
        "                *para_ref = scrubbed;\n"
        "                for scheme in dropped {\n"
        "                    if !self.dropped_schemes.contains(&scheme) {\n"
        "                        eprintln!(\n"
        '                            "textrill: dropped a link with the {scheme:?} URL scheme; \\\n'
        '                             the text is kept. Pass --allowed_url_schemes {scheme} to allow it."\n'
        "                        );\n"
        "                        self.dropped_schemes.push(scheme);\n"
        "                    }\n"
        "                }\n"
        "            }\n",
        "convert.scrub",
    )


@edit(1, "textrill-gui-rs/tests/acceptance.rs")
def _(t):
    return after(
        t,
        "    let mut seen = std::collections::BTreeSet::new();\n"
        "    for spec in textrill::cli::SPECS {\n"
        "        assert!(\n"
        "            seen.insert(spec.help),\n"
        '            "duplicate help string: {}",\n'
        "            spec.help\n"
        "        );\n"
        "    }\n",
        "    // A11 added --allowed_url_schemes, and with it the panel gained a widget.\n"
        "    // The count is here to make an addition a deliberate act rather than\n"
        "    // something that happens when the engine grows an option.\n",
        "acceptance.note",
    )


# ============================================ level 4: A12 attribute injection into a tag
# A document-supplied `"` must not be able to close an `href` attribute, so the four
# built-in autolink and bare-URL captures stop at a double quote.

@edit(4, "textrill/src/links.rs")
def _(t):
    for old, new, what in (
        (
            r"/&lt;URL:\\s*(\\S+?)\\s*&gt;/ -h-> <A HREF=\"$1\">$1</A>",
            r"/&lt;URL:\\s*([^\\s\"]+?)\\s*&gt;/ -h-> <A HREF=\"$1\">$1</A>",
            "url",
        ),
        (
            r"/&lt;(http:\\S+?)\\s*&gt;/ -h-> &lt;<A HREF=\"$1\">$1</A>&gt;",
            r"/&lt;(http:[^\\s\"]+?)\\s*&gt;/ -h-> &lt;<A HREF=\"$1\">$1</A>&gt;",
            "http",
        ),
        (r"|ftp(\\.[\\w\\@:-]+)+/\\S+| -> ftp://$&", r"|ftp(\\.[\\w\\@:-]+)+/[^\\s\"]+| -> ftp://$&", "ftp"),
        (r"|www(\\.[\\w\\@:-]+)+/\\S+| -> http://$&", r"|www(\\.[\\w\\@:-]+)+/[^\\s\"]+| -> http://$&", "www"),
    ):
        t = sub(t, old, new, f"links.a12.{what}")
    return t


# ================================================ level 2: generated-link integrity


@edit(2, "textrill/src/convert.rs")
def _(t):
    t = after(
        t,
        "        let mut pages: Vec<String> = Vec::new();\n",
        "        // The `chunk-N` id of the top-level section each page holds, in page\n"
        "        // order. Needed so a cross-file TOC can point *into* a page rather than\n"
        "        // only at its top, and so `--section` has something to do here.\n"
        "        let mut page_ids: Vec<String> = Vec::new();\n",
        "l2.page_ids_decl",
    )
    t = after(
        t,
        "        let mut current: Option<String> = None;\n",
        "        let mut current_id = String::new();\n",
        "l2.current_id",
    )
    t = after(
        t,
        "                    pages.push(page);\n",
        "                    page_ids.push(std::mem::take(&mut current_id));\n",
        "l2.page_ids_push",
    )
    t = after(
        t,
        "                current = Some(page);\n",
        "                current_id = s.id.clone();\n",
        "l2.current_id_set",
    )
    t = after(
        t,
        "        if let Some(page) = current {\n            pages.push(page);\n",
        "            page_ids.push(current_id);\n",
        "l2.page_ids_push2",
    )
    t = sub(
        t,
        "        } else if !leading.trim().is_empty() {\n            pages.push(leading);\n",
        "        } else if !leading.trim().is_empty() {\n"
        "            // No top-level section at all, which cannot happen while `top` is the\n"
        "            // minimum level present, but a page still needs an id to be a target.\n"
        "            pages.push(leading);\n"
        "            page_ids.push(String::new());\n",
        "l2.page_ids_push3",
    )
    t = sub(
        t,
        "Some(self.render_page_toc(&sections, top, &links))",
        "Some(self.render_page_toc(&sections, top, &links, &page_ids))",
        "l2.toc_call",
    )
    t = sub(
        t,
        "                out.push_str(page);\n                out.push_str(&self.render_pager(&links, i));\n",
        "                // `--section` under `--chunk` used to be silently ignored, because\n"
        "                // a page *is* one top-level section and there was nothing left to\n"
        "                // wrap.\n"
        "                //\n"
        "                // The wrapper is emitted for `--toc` as well as `--section`, and\n"
        "                // that is the point: it is the anchor the TOC's own `file#chunk-N`\n"
        "                // points at. Single-file `--toc` already implies its targets, since\n"
        "                // `sectionize_parts` wraps whenever either flag is set, and a TOC\n"
        "                // that can emit a dangling link is worse than a redundant `<article>`.\n"
        "                if (self.opts.section || self.opts.toc) && !page_ids[i].is_empty() {\n"
        "                    out.push_str(\"<article class=\\\"section\\\" id=\\\"\");\n"
        "                    out.push_str(&page_ids[i]);\n"
        "                    out.push_str(\"\\\">\\n\");\n"
        "                    out.push_str(page);\n"
        "                    if !page.ends_with('\\n') {\n"
        "                        out.push('\\n');\n"
        "                    }\n"
        "                    out.push_str(\"</article>\\n\");\n"
        "                } else {\n"
        "                    out.push_str(page);\n"
        "                }\n"
        "                out.push_str(&self.render_pager(&links, i));\n",
        "l2.article_wrapper",
    )
    t = sub(
        t,
        "        top: usize,\n        names: &[String],\n    ) -> String {\n",
        "        top: usize,\n        names: &[String],\n        page_ids: &[String],\n    ) -> String {\n",
        "l2.render_page_toc_sig",
    )
    return sub(
        t,
        '                out.push_str(&names[page]);\n                out.push_str("\\">");\n',
        '                out.push_str(&names[page]);\n'
        "                // Deep-link into the page. Harmless when the page has no anchor\n"
        "                // of its own, and the fragment is skipped rather than emitted\n"
        "                // empty so the href stays a clean relative path.\n"
        "                if let Some(id) = page_ids.get(page).filter(|id| !id.is_empty()) {\n"
        "                    out.push('#');\n"
        "                    out.push_str(id);\n"
        "                }\n"
        '                out.push_str("\\">");\n',
        "l2.toc_fragment",
    )


@edit(2, "textrill/src/main.rs")
def _(t):
    t = sub(
        t,
        'eprintln!("Error: unable to open {name},: {e}");',
        'eprintln!("Error: unable to open {name}: {e}");',
        "main.comma1",
    )
    t = sub(
        t,
        '        Err(e) => {\n            eprintln!("Error: unable to open {},: {}", opts.outfile, e);\n            false\n',
        '        Err(e) => {\n            eprintln!("Error: unable to open {}: {}", opts.outfile, e);\n            false\n',
        "main.comma2",
    )
    return sub(
        t,
        '            Err(e) => {\n                eprintln!("Error: unable to open {},: {}", opts.outfile, e);\n                return ExitCode::from(1);\n',
        '            Err(e) => {\n                eprintln!("Error: unable to open {}: {}", opts.outfile, e);\n                return ExitCode::from(1);\n',
        "main.comma3",
    )


# ================================= level 3: opt-in citations and glossary


@edit(3, "textrill/src/lib.rs")
def _(t):
    return after(t, "pub mod links;\n", "pub mod notes;\n", "lib.notes_mod")


@edit(3, "textrill/src/cli.rs")
def _(t):
    t = after(
        t,
        '    Flag "Write one HTML file per top-level section (P5.2)." ["chunk"],\n',
        '    Flag "Collect {{textrill:cite:key}} references into a numbered endnotes list." '
        '["citations", "notes"],\n',
        "cli.citations_spec",
    )
    t = after(
        t,
        '    Flag "Emit HTML5: <!DOCTYPE html> and a charset meta (P5.1)." ["html5"],\n',
        '    Flag "Collect {{textrill:gloss:term}} references into a definition list." ["glossary"],\n',
        "cli.glossary_spec",
    )
    t = after(
        t,
        '        "chunk" => opts.chunk.to_string(),\n',
        '        "citations" => opts.citations.to_string(),\n',
        "cli.get_citations",
    )
    t = after(
        t,
        '        "hrule_min" => opts.hrule_min.to_string(),\n',
        '        "glossary" => opts.glossary.to_string(),\n',
        "cli.get_glossary",
    )
    return after(
        t,
        '        "chunk" => opts.chunk = value,\n',
        "        // Note and glossary collection.\n"
        '        "citations" => opts.citations = value,\n'
        '        "glossary" => opts.glossary = value,\n',
        "cli.set_bool",
    )


@edit(3, "textrill/src/convert.rs")
def _(t):
    t = after(
        t,
        "    template_text: String,\n",
        "    /// Set when a note or glossary set is unusable, so the caller can report it\n"
        "    /// and write nothing.\n"
        "    ///\n"
        "    /// Conversion cannot return this as an error without changing the signature\n"
        "    /// the reference and the Python bindings use, and it must not be reported by\n"
        "    /// printing and carrying on: a document whose citations lost a definition\n"
        '    /// would otherwise reach disk with a dangling `[1]`.\n'
        "    pub notes_error: Option<String>,\n",
        "cv3.notes_error_field",
    )
    t = after(
        t,
        "        Converter {\n",
        "            notes_error: None,\n",
        "cv3.notes_error_init",
    )
    t = after(
        t,
        "    fn convert_sources(&mut self, sources: Vec<String>, string_mode: bool) -> String {\n",
        "        self.notes_error = None;\n",
        "cv3.notes_error_reset",
    )
    t = before(
        t,
        "        // P5.5. A template replaces the default arrangement. `--template`\n",
        "        // Collect the note sets over the finished body, so definition content\n"
        "        // is rendered markup rather than source text. On failure the body is\n"
        "        // left exactly as it was and the error is recorded: the caller writes\n"
        "        // nothing, so a refused document produces no output at all.\n"
        "        let (stripped, citations, glossary) = match self.collect_notes(&body) {\n"
        "            Ok(n) => n,\n"
        "            Err(e) => {\n"
        "                self.notes_error = Some(e);\n"
        "                (String::new(), String::new(), String::new())\n"
        "            }\n"
        "        };\n"
        "        let body = if stripped.is_empty() { body } else { stripped };\n",
        "cv3.collect_call",
    )
    t = sub(
        t,
        "            return self.apply_template(&start, &body, &toc, &tail);\n",
        "            return self.apply_template(&start, &body, &toc, &tail, &citations, &glossary);\n",
        "cv3.apply_call",
    )
    t = before(
        t,
        "        let mut out = String::with_capacity(start.len() + toc.len() + body.len() + tail.len());\n",
        "        let mut body = body;\n"
        "        // Without a template there is nowhere to put the lists, so they go at\n"
        "        // the end of the body -- after the prose, where a reader expects an\n"
        "        // endnotes section, and inside `--extract`'s output too.\n"
        "        body.push_str(&citations);\n"
        "        body.push_str(&glossary);\n",
        "cv3.tail_push",
    )
    t = before(
        t,
        "    /// P5.5. Fill the active template's slots and assemble the page.\n",
        "    /// Collect and validate the note sets, returning the finished lists.\n"
        "    ///\n"
        "    /// Returns `(stripped body, citations HTML, glossary HTML)`. All three are\n"
        "    /// empty when both modes are off, which is the default and must leave the\n"
        "    /// body untouched.\n"
        "    fn collect_notes(&self, body: &str) -> Result<(String, String, String), String> {\n"
        "        if !self.opts.citations && !self.opts.glossary {\n"
        "            return Ok((String::new(), String::new(), String::new()));\n"
        "        }\n"
        "        let (stripped, notes) =\n"
        "            crate::notes::collect(body, (self.opts.citations, self.opts.glossary));\n"
        "        crate::notes::validate(&notes)?;\n"
        "        Ok((\n"
        "            stripped,\n"
        "            notes.render_one(crate::notes::Kind::Citation),\n"
        "            notes.render_one(crate::notes::Kind::Glossary),\n"
        "        ))\n"
        "    }\n"
        "\n",
        "cv3.collect_fn",
    )
    t = sub(
        t,
        "    fn apply_template(&self, start: &str, body: &str, toc: &str, tail: &str) -> String {\n"
        "        let slots = [\n"
        '            ("content", body),\n',
        "    fn apply_template(\n"
        "        &self,\n"
        "        start: &str,\n"
        "        body: &str,\n"
        "        toc: &str,\n"
        "        tail: &str,\n"
        "        citations: &str,\n"
        "        glossary: &str,\n"
        "    ) -> String {\n"
        "        // A template that names neither slot would silently drop the notes, so\n"
        "        // the fallback is the same end-of-body placement the untemplated path\n"
        "        // uses. Falling back keeps a template working without change when the\n"
        "        // author has no notes; requiring the slots would not.\n"
        "        let has_slot = |name: &str| {\n"
        "            self.template_text\n"
        '                .contains(&format!("{{{{textrill:{name}}}}}"))\n'
        "        };\n"
        "        let mut body = body.to_string();\n"
        '        if !citations.is_empty() && !has_slot("citations") {\n'
        "            body.push_str(citations);\n"
        "        }\n"
        '        if !glossary.is_empty() && !has_slot("glossary") {\n'
        "            body.push_str(glossary);\n"
        "        }\n"
        "        let slots = [\n"
        '            ("content", body.as_str()),\n',
        "cv3.apply_sig",
    )
    return after(
        t,
        '            ("pager", ""),\n',
        '            ("citations", citations),\n            ("glossary", glossary),\n',
        "cv3.slots",
    )


@edit(3, "textrill/src/main.rs")
def _(t):
    return after(
        t,
        "        Err(e) => (e.out, e.unreadable),\n    };\n",
        "\n"
        "    // A note set that cannot be rendered is a failure, and it is checked before\n"
        "    // the output is opened. Writing the body anyway would leave `[1]` in the\n"
        "    // prose pointing at nothing, which is exactly the failure the mode exists\n"
        "    // to prevent -- and a half-converted document on disk is worse than none.\n"
        "    if let Some(e) = &conv.notes_error {\n"
        '        eprintln!("{PROG}: {e}");\n'
        "        return ExitCode::from(1);\n"
        "    }\n",
        "mn3.notes_check",
    )


# ------------------------------------------------------------------- build


# ==================================================================== documentation

@edit(1, "textrill/README.md")
def _(t):
    t = sub(
        t,
        "  differential corpus of 59 cases and 33 upstream golden files.\n",
        "  differential corpus of 60 cases and 33 upstream golden files.\n",
        "readme.corpus_count",
    )
    t = sub(
        t,
        "- 222 Rust tests, a fuzzer, and a 74-test native GUI suite.\n",
        "- @@README_TESTS@@ Rust tests, a fuzzer, and a 74-test native GUI suite.\n",
        "readme.test_count",
    )
    return after(
        t,
        "| `--default_link_dict` | none | Load a link dictionary, as the original does |\n",
        "| `--allowed_url_schemes` | none | Allow only these URL schemes in `href`s, "
        "instead of refusing the script-bearing ones |\n",
        "readme.option_row",
    )


@edit(2, "textrill/README.md")
def _(t):
    return sub(
        t,
        "carries the same TOC (with cross-file links when `--toc` is set) and prev/next\n"
        "pager links. `--chunk` cannot be combined with `--extract`, `--instring`, or\n"
        "output to standard output.\n",
        "carries the same TOC and prev/next pager links. `--chunk` cannot be combined\n"
        "with `--extract`, `--instring`, or output to standard output.\n"
        "\n"
        'Each chunked page is itself wrapped in `<article class="section" id="chunk-N">`,\n'
        "so a cross-file TOC entry deep-links to the heading rather than to the top of the\n"
        "page: `out-chunk-02.html#chunk-3`. The wrapper is emitted for `--toc` as well as\n"
        "`--section`, because it is the anchor the TOC points at \u2014 `--toc` implies its\n"
        "own targets, so it can never emit a link to somewhere that is not there.\n"
        "\n"
        "Every internal link the engine generates is checked to resolve, in\n"
        "`tests/linkintegrity.rs`. Note the scope: a document can write its own URLs with\n"
        "`<URL:...>` or a link dictionary, and those are passed through as the author\n"
        "wrote them \u2014 the converter has no way to know whether `docs/readme` exists.\n",
        "readme.chunk_ids",
    )


@edit(3, "textrill/README.md")
def _(t):
    t = after(
        t,
        "  explicit overrides for the encodings that cannot be detected.\n",
        "- Opt-in `--citations` and `--glossary` collect namespaced markers into an\n"
        "  endnotes list and a definition list, with no JavaScript and no inference \u2014 see\n"
        "  [Citations and glossary](#citations-and-glossary). Generated `href`s and their\n"
        "  target ids are checked by a permanent test, as are the `href`s built by\n"
        "  `--toc`, `--section` and `--chunk`.\n",
        "readme.feature_bullet",
    )
    t = sub(
        t,
        "There are **62 options** with **116 accepted spellings** including short\n",
        "There are **65 options** with **121 accepted spellings** including short\n",
        "readme.option_count",
    )
    return after(
        t,
        "Because a template is an `Options` value, it can be set once in\n"
        "`./.txt2htmlrc`, so a project commits its template and points at it there.\n",
        "\n" + CITATIONS_SECTION,
        "readme.citations_section",
    )


@edit(1, "textrill/tests/linktest.rs")
def _(t):
    # The parser now takes the scheme policy; four unit tests construct one directly.
    return subn(
        t,
        "links::LinkParser::new(false)",
        "links::LinkParser::new(false, textrill::urlscheme::UrlPolicy::default())",
        4,
        "linktest.parser_ctor",
    )


@edit(1, "textrill/tests/corpus/cases.sh")
def _(t):
    return after(t, "NOGOLDEN[opt_injection]='differential must fail: deliberate Tier 2 divergence: the reference interpolates --title and --style_url into the document unescaped, which is a live XSS. The port escapes them, so a byte comparison against the reference must fail and cannot be the oracle; the oracle is the XML well-formedness check in proptest.py'\n", "\n" +  '# --- A11: the URL scheme policy on generated hrefs ---\n#\n# A document can write a live `javascript:` or `data:` URL into its own output\n# with a <URL:...> tag, and the reference does exactly that: the port\'s\n# pre-A11 output for this input contained href="javascript:alert(document.domain)".\n# The engine now refuses those schemes, unwraps the anchor and keeps the text.\n#\n# So this is the same shape as opt_injection above and for the same reason -- a\n# deliberate Tier 2 divergence from a reference defect, which means the\n# differential comparison CANNOT be the oracle. Saying so explicitly is the\n# point of the "differential must fail:" prefix.\n#\n# The payload characters matter and are not arbitrary. The system dictionary has\n# two <URL:...> rules; the first, <URL:foo:label>, only matches a label built\n# from [a-zA-Z0-9\'() ], and it splits the scheme off as the href. Put a \'.\', \'/\'\n# or \';\' in the label and that rule stops matching, the second rule takes the\n# whole string as the href, and the scheme reaches the output. Every payload\n# below carries one, so the case exercises the hole rather than the near miss.\n# The last reference, <URL:javascript:alert(1)>, is the near miss and is in the\n# input on purpose: the first rule matches it, the href is the relative word\n# "javascript", and it must be left alone. Refusing it would be the policy\n# pattern-matching a substring rather than reading a URL.\nEXTRA[url_scheme]=\'extract=>1,make_links=>1\'\nCLI[url_scheme]=\'--extract --make_links\'\nINPUT[url_scheme]="$HERE/inputs/url_scheme.txt"\nNOGOLDEN[url_scheme]=\'differential must fail: A11, a deliberate Tier 2 divergence where the port is unambiguously better. The reference emits whatever scheme a <URL:...> tag names, so this input came out of it with a live href="javascript:alert(document.domain)" and a live data: URL. The port refuses every scheme outside --allowed_url_schemes, unwraps the anchor and keeps the text, so a byte comparison against the reference must fail and cannot be the oracle. The oracle is tests/urlschemetest.rs, which asserts no refused scheme survives, that the words are kept, and that the relative href the label spelling produces is left alone\'\n'
, "cases.url_scheme")


@edit(3, "textrill/src/template.rs")
def _(t):
    return sub(
        t,
        'pub const SLOTS: &[&str] = &["content", "toc", "title", "head", "pager"];\n',
        'pub const SLOTS: &[&str] = &[\n'
        '    "content",\n'
        '    "toc",\n'
        '    "title",\n'
        '    "head",\n'
        '    "pager",\n'
        '    "citations",\n'
        '    "glossary",\n'
        '];\n',
        "template.slots",
    )


# ------------------------------------------------------------ REMEDIATION-PLAN.md
# The four additions sit in one contiguous run of the document (notes, A12, A11,
# link integrity), so the whole run is rebuilt from the saved final text by level
# rather than inserted piecewise: the sections share surrounding context, and a
# piecewise insert would have to reproduce it.


PLAN_RUN_HEAD = '- **A2 is the exception to "the harness will show you".** It is the one item\n  whose defect is invisible to byte-comparison, so it is verified by P12\'s\n  allocation budget instead, and its current figures are unverified.\n\n'
PLAN_RUN_TAIL = "### Security meta \u2014 design (2026-10-04, boundary notes)\n"

REGION_ORDER = (3, 1, 4, 2)

REGIONS = {
    1: '### A11. A document can write a live `javascript:` href into its own output\n\nFound while writing the security-meta design note below, rather than by the\nattack pass — the plan said "a boundary condition to enforce *if/when expanding\nautolinking*", and it turns out the condition is already met by the existing\n`--make_links`. Severity Low, and Low for a specific reason: this needs the\nvictim to convert a hostile file *and* serve the result. The blast radius is one\nvisitor\'s session, not the converter\'s.\n\n`links.rs:832` is `/<URL:\\s*(\\S+?)\\s*>/ -h-> <A HREF="$1">$1</A>`. The\ndocument names the URL and it is interpolated into `href` unchecked, so\n`<URL:javascript:alert(document.domain)>` in an ordinary-looking file came out\nof the reference as a live link. `data:` is the same, and worse as a navigation\ntarget. `links.rs:829`, `/<URL:(…):([a-zA-Z0-9\'() ]+)>/`, is worse still: `$1`\nis `[-\\w\\.\\/:~_\\@]+`, which *contains* `:`, so\n`<URL:javascript:x:label>` yields `href="javascript:x"` through the label rule.\nA dictionary can name any URL too, and `--style_url` is operator input (that one\nwas already escaped by A8, but escaped and dangerous are different things).\n\n**Done.** `src/urlscheme.rs` is the single decision point, and it is a scan over\n*finished markup* rather than a check per construction site, so a producer\nnobody thought about is covered too. Refused anchors are unwrapped, not deleted —\na hostile document must not be able to deny service by making the converter fail.\nA refused dictionary URL is a load-time diagnostic, because the operator can fix\nthat one.\n\nThree decisions that were not obvious:\n\n* **The default is a four-scheme denylist, not an allowlist.** The plan asked for\n  a strict allowlist, and it is implemented — that is what\n  `--allowed_url_schemes` does. Making it the *default* breaks real input,\n  upstream\'s own CI fixture being the proof: `.github/workflows/xyz.dict` is one\n  rule, `|xyz:[\\w/\\.:+\\-]+| -> $&`, and its smoke test links `xyz://example.com`.\n  No allowlist survives that but "include `xyz`", and `xyz` is not a scheme\n  anyone could have predicted. An unknown scheme is also not an *executing* one:\n  a browser shown `xyz://example.com` does nothing. So the default answers the\n  question that matters — which schemes do something — and the four named ones\n  are `javascript`, `data`, `vbscript`, `file`. The cost is real and is stated\n  in the module docs: a scheme nobody thought of gets through, and\n  `--allowed_url_schemes https` is the mitigation.\n* **`Options::default` carries `None`, and `""` resolves to the default tier.**\n  This replaced an earlier version that spelled the standard list out in the\n  default, which had its own bug: the GUI\'s `get_value`/`set_value` round trip\n  means "unset" has to survive a save/load cycle, and both other encodings were\n  wrong in a different direction — spelling the list out lets it drift from the\n  engine\'s own list, and encoding unset as an empty *allow*list means "allow\n  everything", so a scheme would silently start working again on the next run.\n  `None` → `""` → default tier is the only encoding where the cycle is lossless\n  *and* lands somewhere safe. Tested in both directions.\n* **`rel="noopener noreferrer"` was in the plan\'s boundary note and is not\n  being added.** The note proposed it for external links, but txt2html never\n  emits `target`, so there is no new browsing context for `noopener` to defend\n  and `noreferrer` would only strip referrers the author may want. The usual\n  cargo-culted advice, with nothing to protect here. Adding it would also have\n  changed the bytes of every external link in all 33 goldens.\n\n`url_scheme` is declared `differential must fail:` for the same reason\n`opt_injection` is under A8 — the reference is the defect, so the port *must*\ndiverge and the comparison cannot be the oracle. The real oracle is\n`tests/urlschemetest.rs` (20 cases). Worth recording that `ci_dict` broke on the\nfirst cut of this, which is what forced the denylist decision above.\n\n',
    2: '### Link integrity — investigated 2026-10-04, one defect found and fixed\n\nAsked whether the links textrill *generates* actually work, given that A11 had\njust established the links it *refuses* are handled. Audited, then made\npermanent, because the audit found a coverage hole rather than a broken link.\n\n**Links are correct.** Every configuration tried resolves: single-file\n`#chunk-N` TOC entries, `--extract`, `--number_headings`, `--make_anchors`\nalongside the section ids, the cross-file TOC, and the prev/next pager.\nEscaping in TOC labels is right too — `&`, `<` and `"` survive, and inline\nmarkup inside a heading is stripped from the label but kept in the heading.\nThe pager\'s *targets* and the TOC\'s *labels* were both checked against the\npages they name, because a cross-file TOC can link to a file that exists and\nstill be off by one.\n\n**But the corpus covers none of it.** All 60 cases are reference-differential,\nso none passes `--toc`, `--section` or `--chunk` — those are Phase 5 additions\nwith no upstream equivalent to diff against. Running a link checker over the\nwhole corpus output finds **zero** engine-generated links. The code that invents\n`href`s and the `id`s they point at was the least-differentially-covered part of\nthe output, resting on six unit tests in `section.rs`.\n`tests/linkintegrity.rs` is the fix: 12 adversarial documents crossed with the\noption sets that change link structure, single-file and chunked, asserting that\nevery generated internal reference resolves, that ids are unique per document\n(duplicate ids do not break a link — the browser jumps to the first match — so\nthat needs its own assertion), and that each TOC entry names the heading it\npoints at.\n\nTwo mutations confirm the guard can actually fail, which was worth checking\ngiven Phase 0b: a one-character typo in the single-file TOC `href` and a\none-page off-by-one in the cross-file TOC are both caught.\n\n**Defect found: `--section` was silently ignored under `--chunk`,** because a\npage *is* one top-level section, so there was nothing left to wrap. Two\nconsequences, and the second is the one that mattered: the cross-file TOC could\nonly link to the *top* of a page, never to its heading.\n\nNow each page carries `<article class="section" id="chunk-N">` and the TOC links\n`page.html#chunk-N`. The wrapper is emitted for `--toc` as well as `--section`,\nwhich is the important half: single-file `--toc` already implies its targets,\nsince `sectionize_parts` wraps whenever either flag is set, and **a TOC that can\nemit a dangling link is worse than a redundant `<article>`**. The first cut got\nthis wrong — it emitted the fragment only under `--section`, so `--toc` alone\nproduced `page.html#chunk-N` with no `chunk-N` on the page, which\n`linkintegrity.rs` caught immediately. Chunk mode has no upstream equivalent and\nno golden, so nothing else would have.\n\n**Scope note, because it is the part that is easy to get wrong.** A document can\nwrite its own URLs through `<URL:...>` or a link dictionary, and those are the\nauthor\'s claims about the world: the converter emits `docs/readme` faithfully and\ncannot know the file exists. Holding those to a file-existence rule tests the\ninput, not the output. The guard is scoped to the `<nav>` blocks the engine\nbuilds. External URLs are out of scope, and so are resource references — a\n`<link href>` to a stylesheet that 404s is a missing asset, not a dead link.\n\nAlso fixed: three error messages in `main.rs` had a stray comma in the format\nstring, so a write failure read `unable to open out.html,: ...`.\n\n',
    3: '### Citations and glossary (2026-10-04; implemented)\n\n**Status: implemented.** `--citations` and `--glossary`, both default-off. With\nboth off the output is byte-identical to the behaviour before this section, which\nthe corpus and the 33 goldens confirm rather than assume.\n\n#### Principles (from FINDINGS.md §2)\n- **No inference.** Ordinary prose is never read as a citation or a term. Only\n  an explicit, namespaced marker family is interpreted. `[^1]`, `^2`, `(3)`,\n  `[4]`, `@five`, `{6}` and `~x~` are all text, and `tests/notest.rs` asserts a\n  document built from them converts identically with the modes on and off.\n- **Zero JavaScript.** Satisfied more strongly than the original sketch\n  required: there is no CSS to reveal either, because the note body is emitted\n  once as a real list rather than once per reference.\n- **Fail closed.** A dangling reference, an orphan definition, a duplicate\n  definition, an empty definition, an unbalanced or mismatched block, an invalid\n  key, or any well-formed `textrill:` token we cannot read is an error, and\n  `main.rs` reports it and exits non-zero **before the output file is opened**.\n  A refused document therefore produces no output at all.\n- **No collision with existing features.** The pass runs over the finished body,\n  after numbering and sectioning, and touches nothing else. It composes with\n  `--number_headings`, `--section`, `--toc`, `--extract`, `--html5`,\n  `--lower_case_tags` and templates; `tests/notest.rs` runs the same document\n  through each combination and checks the note links still resolve.\n- **Security boundary respected.** Definitions produce only same-document\n  fragments (`#note-…`), so no scheme is ever chosen from document text and\n  `javascript:` is not reachable by construction. The keys land in `id`\n  attributes, so the key charset is an injection boundary and is restricted to\n  ASCII letters, digits, `-`, `_` and `.`.\n\n#### Flags\n- `--citations` (alias `--notes`): collect citations into a numbered endnotes list.\n- `--glossary`: collect terms into a definition list.\n\nThe two are independent. A marker whose mode is off stays literal text, so\nturning one on never draws the other in and never refuses a document over\nmarkers the user did not ask to be interpreted.\n\nRefused combinations, reported up front by `Options::validate` before any byte is\nwritten: `--chunk` and `--stream`. Numbering follows first reference, so a\ncitation in the last paragraph can insert `[1]` in the first; both modes write as\nthey go, and neither can afford the document-wide pass the feature needs.\n`--extract` is **supported**, not refused — the lists append to the body, which\nfor `--extract` is the whole output, so nothing is lost.\n\n#### Syntax (namespaced, balanced)\n- Citation reference: `{{textrill:cite:key}}`\n- Citation definition: `{{textrill:def:cite:key}}` … `{{/textrill:def:cite:key}}`\n- Glossary reference: `{{textrill:gloss:key}}`\n- Glossary definition: `{{textrill:def:gloss:key}}` … `{{/textrill:def:gloss:key}}`\n\nBalanced tokens rather than the single-token or checkbox shapes sketched here,\nand the reason is a correctness one rather than a taste one: a closing tag is\nchecked against the block it closes, so a typo cannot end the wrong block and\nnest the next one inside it.\n\nA `{{textrill:` with no `}}` is **not** a token — it is text, and stays text.\nThat is the one deliberate exception to "unknown marker → error", because such a\nstring renders exactly as written and refusing a document over it would be\nstrictly worse than leaving it alone.\n\n#### Collection and pass model\n1. Convert as today to body HTML, then number headings and section.\n2. If both modes are off: stop here. The body is untouched.\n3. Otherwise scan the body for the four tokens. The scan is a byte walk, not a\n   line walk, because a marker in running prose is the normal case.\n4. Collect references and definitions separately. Tracked as two independent\n   flags on each entry, because the two failure directions are different\n   mistakes with different fixes: a reference with no definition is a lost\n   citation, and a definition nothing references is usually a key typo that will\n   *become* a lost citation. Collapsing them into one flag loses whichever half.\n5. Validate, and on any error record the diagnostic and emit nothing.\n6. Render and place.\n\nThe pass runs over the **rendered** body, so definition content is already-\nconverted markup: a definition may use textrill\'s own delimiters (`*italic*`\nbecomes `<em>`), and raw HTML a type `<em>` in the source is escaped, exactly as\nit would be anywhere else in the document.\n\nKnown limitation: because the pass runs over the body rather than the source,\n`{{` inside `<pre>`/`<code>` can be collected. Authors keep the modes off for\nsuch a file, or the marker is simply not present.\n\n#### Output structure (and why not the CSS reveal)\n- Citations: `<a class="note-ref" href="#note-key">[n]</a>` inline, then one\n  `<section class="notes"><ol class="notes-list">` with `<li id="note-key">`.\n  `<ol>` because the visible label is a number, which is what an ordered list\n  renders by default.\n- Glossary: `<a class="gloss-ref" href="#gloss-key">key</a>` inline, then\n  `<section class="glossary"><dl class="glossary-list">` with\n  `<dt id="gloss-key">key</dt><dd>`. `<dl>` because the term is authored text,\n  not a number, and a generated marker in a `<dt>` would misdescribe the source.\n- Each note ends with a "back" link to the first reference.\n\nThe CSS-only reveal sketched above would have to emit the definition body **once\nper reference** to make a later one revealable, so a citation cited five times\nwould appear five times in the DOM — which then means the wrong copy for\nprinting, for `--extract`, and for a reader who never expands anything. The\ndefinition list keeps one copy, works with CSS off, prints correctly, and needs\nno JavaScript. That is why the original sketch was dropped.\n\nOnly the **first** reference to a key carries an `id`. Repeating the id on every\nmention would be invalid HTML and would leave the "back" link with several\npossible targets; suffixing each would leave every mention but the first\nunaddressable. `tests/notest.rs` asserts 50 references to one key produce exactly\none `id="note-ref-a"` and one `id="note-a"`.\n\n#### Interactions & constraints\n- **Templates:** `{{textrill:citations}}` and `{{textrill:glossary}}` place the\n  two lists independently. A template naming neither slot falls back to the same\n  end-of-body placement, so a template written before this feature keeps working\n  and never loses a note list. Unknown `textrill:` slots remain hard errors.\n- **Section/TOC:** unaffected; `tests/notest.rs` checks the `chunk-N` ids and the\n  note ids coexist without collision.\n- **Encoding:** markers are ASCII; keys are restricted to ASCII, so a key can\n  never split a UTF-8 sequence.\n\n#### Non-goals\n- No markdown-style implicit footnotes; no heuristic inference.\n- No JavaScript, and no CSS to go with it.\n- Not a bibliography processor (citeproc): inline references plus a list, nothing\n  more.\n\n',
    4: '### A12. A document can inject an attribute into a tag the engine generated\n\nFound while auditing the decision to omit `rel="noopener noreferrer"` from the\ncitation/glossary links, when the question asked was the broader one: *can a\ndocument subvert a generated link at all?* It can, and not through the notes\ncode. Severity High, and higher than A11\'s, because unlike A11 it needs no\n`javascript:` URL and no cooperation from any option: the default\n`--make_links` is enough.\n\n**The gap.** The engine escapes `&`, `<` and `>` in document text. It does not\nescape `"`, which is correct for prose — a double quote is a printable character\nand never needs escaping in running text. But the autolinker writes what it\ncaptures into `HREF="$1"`, and four built-in rules captured `\\S+`, which\nadmits `"`:\n\n- `/<URL:\\s*(\\S+?)\\s*>/` (`links.rs`)\n- `/<(http:\\S+?)\\s*>/`\n- `|ftp(\\.[\\w\\@:-]+)+/\\S+|`\n- `|www(\\.[\\w\\@:-]+)+/\\S+|`\n\nSo a document containing\n\n```\n<URL:x"onmouseover="alert(1)>\n```\n\nconverted to\n\n```html\n<a href="x"onmouseover="alert(1)">x"onmouseover="alert(1)</a>\n```\n\nA live event handler on a tag the engine emitted. Hover the link and it runs in\nthe origin serving the converted document. `target="..."` works the same way,\nwhich is the direct answer to the `noopener` question: the reason that attribute\nis absent is not only that no link opens a new context, it is that a document\ncould not add one — until this fix it could.\n\n**Why A11 did not catch it.** The scheme scrubber inspects the *value* of an\n`href`. Here the value is `x`, which is a legal relative reference and passes.\nThe damage is in the attribute *syntax* around the value, which no amount of\nscheme checking looks at. The lesson recorded for the next pass: a URL policy\nmust constrain what may appear in an attribute value, not only judge the value\nit finds.\n\n**Fix.** The four captures now exclude `"` (`[^\\s"]+`). Chosen over escaping\nthe expanded value because it changes output only in the case that was a\nvulnerability, so every legitimate URL stays byte-identical and the corpus and\n33 goldens still pass unchanged.\n\n**Not a fix, and recorded as such:** a `-h` link-dictionary rule may still emit\nwhatever attributes it likes. That is operator input, not document input, so it\nis out of this threat model — but it does mean `rel="noopener noreferrer"`\nremains the operator\'s responsibility if they write `target=` by hand.\n\n**Guard.** `tests/urlschemetest.rs` parses every generated `<a …>` tag across\nten injection payloads and eight option combinations, and asserts no attribute\nis an event handler and none is `target`. Reverting the `\\S` fix makes it fail,\nwhich is the only evidence a test like that is worth anything.\n\n',
}

# Appended to the level-4 region: recorded in this series, so it exists only at the
# final level, like the A11/A12 sections above it.
PLAN_P57 = '''

### Budget-driven page boundaries — proposed (P5.7), **not implemented**

**The ask.** Given a text file, count its characters including whitespace, then
pick the heading level that `--chunk` should split at so that no page holds more
than a chosen budget of text. Useful because a page is currently as large as its
top-level section happens to be, and nobody knows that number until they have
already converted.

**Counting is not the hard part.** `try_convert_chunked` (`convert.rs`) reads
every `--infile` into a `String` through `read_with` before conversion starts, so
the source is fully materialised and counting it is free. `assemble_chunked`
already holds a `Vec<Section>` carrying both `level` and `html`, so candidate
page sizes per heading level are a few dozen lines away. Choosing the cut is the
whole of the design.

**Why this is not simply a sort-and-split, and cannot always succeed:**

* **Page boundaries are heading boundaries.** The sectioner never splits inside
  a section, so one section larger than the budget produces an oversized page at
  *every* level. No choice of level helps. The budget is therefore a request, and
  an implementation must say out loud that it was not met, and name the section
  that overflowed. Reporting a silent overflow would be worse than having no
  budget at all, because the user asked for a guarantee.
* **Cutting at exactly level L is not monotone.** If the document has no heading
  at level L, that level yields a single page holding the entire body — strictly
  larger than cutting one level up. Walking levels in search of "coarsest cut that
  fits" can therefore step past a level that fitted onto one that does not.
  Cutting at every heading of level *or shallower* restores monotonicity at the
  cost of an orphan page holding a heading's preamble. That is a real choice, not
  an implementation detail, and it has not been made.
* **Only levels the document actually contains** should be candidates, or the
  report describes cut strategies that cannot happen.
* **Characters, bytes, or grapheme clusters.** `str::len` is bytes,
  `chars().count` is Unicode scalars, and neither is what a reader counts. On
  this project's own inputs the three disagree, which is the same reason
  `fix/non-ascii-delimiter-predicate` exists.
* **Source characters, body HTML, or output bytes are three different numbers.**
  Each page is `start + toc + page + tail`, so the template is paid *per page*,
  and HTML inflates the body unpredictably. "How much text went into each
  template" needs to be its own figure or the two will never agree and the feature
  will read as broken.

**What a report should print.** Per page: index, output bytes, source characters,
cut level. Then pages, and min/median/max. Median rather than mean, because one
enormous section drags a mean to nothing — and that section is exactly the case
the reader needs to see.

**Measure first.** The natural first step is a report-only flag that changes no
behaviour. That is not a way of avoiding the decision; it is how the decision gets
made, because nothing in this repository currently measures the section-size
distribution of any real input. Whether the budget can do anything at all depends
on whether real documents are heading-uniform, and that is unknown here.

**Not proposed.** Splitting by paragraph count, which would contradict the settled
heading-based decision recorded under *Sectioning and TOC — design*. And failing
the conversion when the budget cannot be met: the size of one section is a fact
about the input, not an error in it, so it must not turn a document into a
non-zero exit.
'''

REGIONS[4] = REGIONS[4].rstrip("\n") + "\n\n" + PLAN_P57.strip("\n") + "\n\n"

PLAN_ORACLE = '''### Oracle coverage: which options have one — audited 2026-10-06

Derived mechanically so that it can be re-derived, because a coverage claim
nobody can reproduce is a coverage rumour. `cli.rs` `SPECS` gives the port side
(65 options, 121 names counting aliases). The `GetOptions` block in
`ref/txt2html-3.0/scripts/txt2html` gives what the reference implements (58 spec
entries, 113 names). The two oracles are the 60 differential cases and the
seeded fuzzer; both were read by sourcing `cases.sh` and scanning `fuzz.py`, not
by grepping for option names, because a grep cannot tell `--escape_HTML_chars`
from an option that is merely never mentioned.

| class | n | what it means |
| --- | --- | --- |
| explicit differential oracle | 45 | varied by a corpus case or a fuzz seed, then compared byte for byte against Perl |
| no reference equivalent | 13 | txt2tags has no equivalent, so nothing can be diffed |
| default path verified only | 3 | `doctype`, `preformat_start_marker`, `preformat_end_marker`: the default value is byte-verified in passing, the non-default branch is not |
| nothing verified | 2 | `append_head`, `prepend_file` |
| plumbing | 1 | `outfile` |
| no-op | 1 | `utf8`, accepted and ignored |

**The 13 are structural, not an oversight.** `html5`, `section`, `toc`, `chunk`,
`number_headings`, `stream`, `template`, `document_template`, `citations`,
`glossary`, `meta_charset`, `encoding`, `allowed_url_schemes`. They rest on the
33 goldens and on hand-written assertions, and a golden pins output against our
own past output: it catches unintended change superbly and cannot catch a wrong
decision that was implemented consistently and then frozen. A11 and A12 are the
argument for taking that limit seriously rather than citing the test count.

**The 2 are a coverage gap, not a correctness one.** Both were checked by hand
against the reference on 2026-10-06 and are byte-identical, so the honest
statement is "correct but unpinned". The fix is two corpus cases.

**A correction, because the first version of this audit was wrong.** Counting
only the 60 cases reported roughly 34 options with no oracle. That was wrong in
both directions: it ignored `fuzz.py`, which varies 30 options and is a real
differential oracle, and it treated "never passed on the command line" as
"unverified", when an option whose default is inert *and* which nothing varies
is the only category that actually means nothing was checked. The number that
survives checking is 2, not 34.'''

REGIONS[4] = REGIONS[4] + PLAN_ORACLE.strip("\n") + "\n\n"

PLAN_PKG = '''### Flatpak packaging — manifest drafted 2026-10-06

`packaging/io.github.example.Textrill.yml` now exists, with `packaging/
rust-stable.sh` and a `make cargo-sources` target. It parses, and the module
order is right — the engine builds first because `textrill-gui-rs` depends on
`../textrill` by path — but **it is not buildable as committed**, for two reasons
that are owed decisions rather than defects:

* **The app-id is a placeholder.** `io.github.example.Textrill`. `example` is
  deliberately not a real owner so that it cannot be shipped by accident. The
  real id is `io.github.<owner>.Textrill`, and `<owner>` is exactly the account in
  the `repository = "https://github.com/<you>/textrill"` TODO carried in both
  `textrill/Cargo.toml` and `textrill-gui-rs/Cargo.toml`. Substituting it means
  changing three places at once — both Cargo.toml files and the manifest filename.
* **`cargo-sources.json` does not exist.** Both modules consume it. It is
  generated rather than committed because it is vendored crate metadata that only
  has to agree with one lockfile, and a stale copy of it is a build failure nobody
  can read. `make cargo-sources` produces it, and refuses to pretend otherwise
  when `flatpak-cargo-generator` is missing — which it is on this host, where
  `flatpak` and `flatpak-builder` are both present.

The vendoring alternative is still undecided. The Flatpak design note above
prefers offline/vendored dependencies, and `cargo vendor` removes the generator
dependency entirely at the cost of a large vendored tree. One or the other has to
be chosen before the first real build.

**Two figures in the Phase 6 tables are historical, not current**, and are left
that way deliberately because they were accurate at the toolkit decision: the
five "kept" engine files were 5,644 lines then and are 7,701 now, and the retired
Python shell was 1,365 lines and is 1,558. A reader comparing them against
`wc -l` today will think the tables are wrong; they are dated numbers without a
date. The dated gate figures at the head of this file are the pattern that works.'''

REGIONS[4] = REGIONS[4] + PLAN_PKG.strip("\n") + "\n\n"

PLAN_AUTOLINK_OLD = """\
- **Autolinking boundary (documented):** `--make_links` and
  `--links_dictionaries` derive hrefs from document text (FINDINGS.md §2). The
  engine must never emit `javascript:`, `data:` (except explicitly allowlisted
  only if ever needed), `vbscript:`, `file:`, or other dangerous schemes. Plan:
  enforce a strict scheme allowlist when autolinking (e.g. `http`, `https`,
  `mailto`, `ftp` if required) and always add `rel="noopener noreferrer"` on
  external links; internal anchors exempt from `rel`. This is a boundary
  condition to enforce if/when expanding autolinking.
"""
PLAN_AUTOLINK_NEW = """\
- **Autolinking boundary: now enforced, as A11.** The note below originally said
  this was "a boundary condition to enforce if/when expanding autolinking", but
  the existing `--make_links` already met the condition and a document could
  already write a live `javascript:` href into its own output. `src/urlscheme.rs`
  refuses `javascript`, `data`, `vbscript` and `file` by default;
  `--allowed_url_schemes` sets a strict allowlist instead. `rel="noopener
  noreferrer"` was proposed here and is deliberately **not** added — see A11 for
  why it protects nothing here.
"""
PLAN_NOTESBULLET_OLD = """\
- **Citations/notes:** if they produce hrefs, same rules apply. CSS-only reveal
  means no script-based navigation.
"""
PLAN_NOTESBULLET_NEW = """\
- **Citations/notes:** implemented, and the rule holds by construction — they
  emit same-document fragments (`#note-…`), never a URL, so no scheme is ever
  chosen from document text. Their keys land in `id` attributes, so the key
  charset is an injection boundary and is restricted to ASCII letters, digits,
  `-`, `_` and `.`.
- **Attribute injection is the sharper boundary, not the scheme.** A document
  that cannot name a dangerous scheme can still end the `href="…"` attribute and
  supply its own `onmouseover=`. That was live in four built-in autolink rules
  and is now fixed and guarded — see the section above. Any future URL-emitting
  feature needs the same check, because `rel="noopener noreferrer"` and the
  scheme policy both do nothing about it.
"""

_REGION_FIXUPS = (
    (1,
     "### A11. A document can write a live `javascript:` href into its own output",
     "### A11. A document can write a live `javascript:`/`data:` href into its own output"),
    (2,
     "`tests/linkintegrity.rs` is the fix: 12 adversarial documents crossed with the",
     "`tests/linkintegrity.rs` is the fix: 10 adversarial documents crossed with the"),
    (4,
     "### A12. A document can inject an attribute into a tag the engine generated",
     "### A12. A document can inject an attribute into an anchor the engine generated"),
    (2,
     "resting on six unit tests in `section.rs`",
     "resting on ten unit tests in `section.rs`"),
)
for _lvl, _old, _new in _REGION_FIXUPS:
    REGIONS[_lvl] = subn(REGIONS[_lvl], _old, _new, 1, f"regions[{_lvl}].fixup")

# The design-decision paragraph in the security-meta area, rewritten from "deferred"
# to "implemented". Both sides are taken verbatim from HEAD and the saved final.
PLAN_DESIGN_OLD = """\
   **Citations and glossary (design decision, 2026-10-04):** deferred by default.
   If built, these will be **opt-in only**, behind a new flag (or flags), and
   triggered exclusively by a **collision-proof, namespaced sigil** (not `^1`,
   not `[^1]`). Reveal must be **CSS-only** (checkbox/label) with no JavaScript.
   Definitions/citations are collected in their own pass and any
   dangling/duplicate/empty/ambiguous reference is a **hard error** when the
   mode is active; when off and unused, output must remain byte-identical to
   current behaviour. They will not reuse the `{{...}}` forms in a way that
   conflicts with templates except by a distinct sub-namespace under
   `textrill:`; autolinking safety (scheme allowlisting + `rel="noopener
   noreferrer"`) remains a boundary condition if hrefs are produced. This is
   design-only; no code change yet.
"""
PLAN_DESIGN_NEW = """\
   **Citations and glossary (2026-10-04):** now **implemented**, as
   `--citations` and `--glossary`, both **opt-in and default-off**, triggered
   exclusively by **collision-proof, namespaced** markers (not `^1`, not `[^1]`).
   Collected in their own pass over the finished body; any
   dangling/duplicate/empty/ambiguous reference is a **hard error** when the mode
   is active, reported before the output file is opened. When off and unused,
   output is byte-identical, which the corpus and 33 goldens check. They do not
   reuse the `{{...}}` forms in a way that conflicts with templates, only a
   distinct sub-namespace under `textrill:`. Generated links are same-document
   fragments, so no scheme is chosen from document text and `rel="noopener
   noreferrer"` is not applicable; the key charset is restricted because keys
   land in `id` attributes. The list is a definition list rather than the
   CSS-only checkbox sketch — see its own section for why duplication made that
   shape wrong.
"""



@edit(1, "REMEDIATION-PLAN.md")
def _(t):
    # Corpus gate count moves with the new `url_scheme` case; the engine-test count
    # is filled in by the measured-count pass.
    t = sub(
        t,
        "Gates at 2026-10-04: corpus **59/59**, goldens **33/33**, **222** engine tests,\n",
        "Gates at 2026-10-04: corpus **60/60**, goldens **33/33**, **@@TESTS@@** engine tests,\n",
        "plan.gates",
    )
    t = sub(
        t,
        "E1\u2013E3, A1, A1b, A2\u2013A10.",
        "E1\u2013E3, A1, A1b, A2\u2013A11.",
        "plan.progress",
    )
    t = sub(
        t,
        "Items are numbered `A1`\u2013`A10` so they do not collide",
        "Items are numbered `A1`\u2013`A11` so they do not collide",
        "plan.anumbering",
    )
    t = after(
        t,
        "| A10 | unbounded `re_cache` | Low | `convert.rs:160` |\n",
        "| A11 | document can write a live `javascript:`/`data:` href into its own output | "
        "**done** (was Low, corrected to High) | `urlscheme.rs`, `links.rs` `<URL:\u2026>`, `options.rs` |\n",
        "plan.a11_row",
    )
    t = sub(t, PLAN_AUTOLINK_OLD, PLAN_AUTOLINK_NEW, "plan.autolink_bullet")
    t = sub(
        t,
        "**Status:** design-only. No code change yet.\n",
        "**Status:** design notes, kept current. The autolinking boundary below is\n"
        "enforced as A11; the remaining boundaries are recorded here as they are\n"
        "settled.\n",
        "plan.security_meta_status",
    )
    t = sub(
        t,
        "The note below originally said\n",
        "This note originally said\n",
        "plan.security_meta_pointer",
    )
    t = sub(
        t,
        "which stays deferred (see the\n   notes policy in FINDINGS.md \u00a72).",
        "which is separate work (see the\n   notes policy in FINDINGS.md \u00a72).",
        "plan.head_slot_note",
    )
    return _plan_run(t, 1)


@edit(1, "ADVERSARIAL-FINDINGS.md")
def _(t):
    return sub(
        t,
        "is the A1\u2013A10 addendum at the end of",
        "is the A1\u2013A11 addendum at the end of",
        "findings.addendum_range",
    )


@edit(2, "REMEDIATION-PLAN.md")
def _(t):
    return _plan_run(t, 2)


@edit(4, "ADVERSARIAL-FINDINGS.md")
def _(t):
    return sub(
        t,
        "is the A1\u2013A11 addendum at the end of",
        "is the A1\u2013A12 addendum at the end of",
        "findings.addendum_range_a12",
    )


@edit(3, "REMEDIATION-PLAN.md")
def _(t):
    t = sub(t, PLAN_DESIGN_OLD, PLAN_DESIGN_NEW, "plan.design_para")
    t = sub(t, PLAN_NOTESBULLET_OLD, PLAN_NOTESBULLET_NEW, "plan.notes_bullet")
    return _plan_run(t, 3)


@edit(4, "REMEDIATION-PLAN.md")
def _(t):
    t = sub(
        t,
        "E1\u2013E3, A1, A1b, A2\u2013A11.",
        "E1\u2013E3, A1, A1b, A2\u2013A12.",
        "plan.progress_a12",
    )
    t = sub(
        t,
        "Items are numbered `A1`\u2013`A11` so they do not collide",
        "Items are numbered `A1`\u2013`A12` so they do not collide",
        "plan.anumbering_a12",
    )
    t = _plan_run(t, 4)
    t = after(
        t,
        "| A11 | document can write a live `javascript:`/`data:` href into its own output | "
        "**done** (was Low, corrected to High) | `urlscheme.rs`, `links.rs` `<URL:\u2026>`, `options.rs` |\n",
        "| A12 | document can inject an attribute into an anchor the engine generated | **done** | "
        "`links.rs` built-in rules, `tests/urlschemetest.rs` |\n",
        "plan.a12_row",
    )
    return t

def _plan_run(t, level):
    """Replace the notes/A12/A11/link-integrity run with every region up to `level`."""
    head, _, tail = t.partition(PLAN_RUN_HEAD)
    if not tail:
        raise SystemExit("plan.run_head: anchor not found")
    _, _, rest = tail.partition(PLAN_RUN_TAIL)
    body = "".join(REGIONS[l] for l in REGION_ORDER if l <= level)
    if level >= 4:
        body = subn(
            body,
            "**Guard.** `tests/urlschemetest.rs` parses every generated `<a \u2026>` tag across\n"
            "ten injection payloads and eight option combinations, and asserts no attribute\n"
            "is an event handler and none is `target`. Reverting the `\\S` fix makes it fail,\n"
            "which is the only evidence a test like that is worth anything.",
            "**Guard.** `tests/urlschemetest.rs` parses the attributes of every generated\n"
            "`<a \u2026>` tag and asserts that none is an event handler and none is `target`.\n"
            "The ten injection payloads are converted with the default `--make_links`; the\n"
            "no-`target` half is then checked again across eight option combinations, one of\n"
            "them turning on every flag at once. Reverting the `\\S` fix makes the first fail,\n"
            "which is the only evidence a test like that is worth anything.",
            1,
            "plan.a12_guard_sentence",
        )
        # A12 grows urlschememetest, and the A11 section is what quotes its size.
        body = subn(
            body,
            "`tests/urlschemetest.rs` (20 cases)",
            "`tests/urlschemetest.rs` (22 cases)",
            1,
            "plan.urlschemetest_count",
        )
    out = head + PLAN_RUN_HEAD + body + PLAN_RUN_TAIL + rest
    # TOOL-SURVEY.md was archived on 2026-10-06: its feature matrix had gone
    # stale -- it still called the port `txt2html-rs` and reported TOC, HTML5,
    # rc files, `charset` and a built-in stylesheet as missing, all of which
    # exist. Rewritten at every level rather than only level 4, so the pointer
    # resolves in each reconstruction and not just the current one.
    out = re.sub(
        r"(?<!legacy-archive/)TOOL-SURVEY\.md",
        "legacy-archive/TOOL-SURVEY.md",
        out,
    )
    old = (
        "Companion documents: `legacy-archive/TOOL-SURVEY.md` (feature-gap survey) and\n"
        "`ADVERSARIAL-FINDINGS.md` (attack pass)."
    )
    new = (
        "Companion documents: `legacy-archive/TOOL-SURVEY.md` (feature-gap survey,\n"
        "archived 2026-10-06 -- its matrix had gone stale) and\n"
        "`ADVERSARIAL-FINDINGS.md` (attack pass, closed). The register of what is\n"
        "current is `DOCS.md`."
    )
    # `_plan_run` is invoked once per level edit, so at level 4 this text is built
    # four times over and the second call sees the already-rewritten form. Accept
    # either, but refuse to pass if neither is present: a silently-skipped anchor
    # would let the document drift back to naming a file that has moved.
    if old in out:
        out = out.replace(old, new)
    elif new not in out:
        raise SystemExit("plan.tool_survey_archived: neither old nor new anchor present")
    return out


@edit(4, "ADVERSARIAL-FINDINGS.md")
def _(t):
    # The pass is closed: every finding A1-A12 is implemented and verified, and the
    # plan's addendum tracks each one. Its scope note already flags the "judge the
    # port against the Perl module as the specification" assumption that is now
    # being retired, so the pointer to the archived survey is corrected rather than
    # dropped -- the option-abbreviation rationale that survey carries is still the
    # reason the divergence is deliberate, and is cited from the plan and README too.
    t = sub(
        t,
        "Status: 2026-09-29. Result of an attack pass over `txt2html-rs` and\n"
        "`txt2html-gui`. Companion to `REMEDIATION-PLAN.md` and `TOOL-SURVEY.md`.",
        "Status: 2026-09-29. **Closed** \u2014 every finding A1\u2013A12 is implemented and\n"
        "verified. Result of an attack pass over `txt2html-rs` and `txt2html-gui`.\n"
        "Companion to `REMEDIATION-PLAN.md`. See `DOCS.md` for its status.",
        "findings.status_closed",
    )
    return sub(
        t,
        "**Option abbreviation absence is deliberate and safe** (see `TOOL-SURVEY.md` \u00a73).",
        "**Option abbreviation absence is deliberate and safe** \u2014 rationale retained in\n"
        "`legacy-archive/TOOL-SURVEY.md`, \u00a7 \"Option abbreviation\": unique-prefix matching\n"
        "breaks silently the day an option is added or renamed.",
        "findings.abbrev_ref",
    )


def main():
    # Built from the pinned base, not HEAD: the levels are painted and committed
    # one at a time, so HEAD moves under us after the first commit.
    texts = {p: git("show", f"{BASE}:{p}") for p in TRACKED}
    for level, path, fn in EDITS:
        if level <= LEVEL:
            texts[path] = fn(texts[path])

    for path, t in texts.items():
        full = os.path.join(REPO, path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        open(full, "w").write(t)

    for path, lvl in WHOLE.items():
        full = os.path.join(REPO, path)
        if lvl > LEVEL:
            if os.path.exists(full):
                os.remove(full)
            continue
        text = open(os.path.join(FINAL, path)).read()
        if path == "textrill/tests/urlschemetest.rs" and LEVEL < 4:
            cut = text.find(A12_MARKER)
            if cut < 0:
                raise SystemExit("urlschemetest.rs: A12 marker missing")
            text = text[:cut].rstrip("\n") + "\n"
        os.makedirs(os.path.dirname(full), exist_ok=True)
        open(full, "w").write(text)

    n_opts, n_spell = COUNTS[LEVEL]
    for path, pat, val in (
        ("textrill/tests/optionstest.rs", r"(cli::SPECS\.len\(\), )\d+", str(n_opts)),
        ("textrill/tests/optionstest.rs", r"(spellings, )\d+", str(n_spell)),
        ("textrill-gui-rs/tests/acceptance.rs", r"(seen\.len\(\), )\d+", str(n_opts)),
    ):
        full = os.path.join(REPO, path)
        t = open(full).read()
        t2, k = re.subn(pat, lambda m: m.group(1) + val, t)
        if k != 1:
            raise SystemExit(f"{path}: {pat} matched {k}x")
        open(full, "w").write(t2)

    # Docs quote the engine-test count, which no per-commit edit can know. Levels 3 and 4
    # add whole test files, so they are counted here rather than guessed; levels 1 and 2
    # only add tests inside existing files, and the final count is carried as a constant
    # that commit 4's own tests are measured against.
    n_tests = COUNTS_ENGINE[LEVEL]
    docs = (
        ("REMEDIATION-PLAN.md", "**@@TESTS@@** engine tests", "**%d** engine tests" % n_tests),
        ("textrill/README.md", "@@README_TESTS@@ Rust tests", "%d Rust tests" % n_tests),
    )
    for path, old, new in docs:
        full = os.path.join(REPO, path)
        t = open(full).read()
        if t.count(old) != 1:
            raise SystemExit(f"{path}: {old!r} occurs {t.count(old)}x")
        open(full, "w").write(t.replace(old, new))

    if LEVEL == 4:
        bad = []
        for root, _, files in os.walk(FINAL):
            for f in files:
                rel = os.path.relpath(os.path.join(root, f), FINAL)
                full = os.path.join(REPO, rel)
                if not os.path.exists(full) or open(full).read() != open(os.path.join(FINAL, rel)).read():
                    bad.append(rel)
        if bad:
            raise SystemExit("level 4 does not reproduce the final tree: " + ", ".join(bad))
    print(f"level {LEVEL} painted")


main()